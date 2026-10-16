'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict'),crypto=require('node:crypto'),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../..'),file='packages/safe-cex/src/exchanges/lighter/lighter.exchange.ts',source=fs.readFileSync(path.join(root,file),'utf8'),tree=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);assert.equal(tree.parseDiagnostics.length,0);
const cls=tree.statements.find(ts.isClassDeclaration),selected=cls.members.filter(n=>n.name?.getText(tree)==='mapOrder');assert.equal(selected.length,1);const original=selected[0].getText(tree);
const bindings=[['v','../../utils/get-key'],['virtualClock','../../utils/virtualClock'],...['OrderType','OrderSide','OrderStatus','OrderTimeInForce'].map(n=>[n,'../../types']),...['ORDER_TYPE','ORDER_SIDE','ORDER_STATUS','ORDER_TIME_IN_FORCE'].map(n=>[n,'./lighter.types'])];
const imports=bindings.map(([name,module])=>{const a=tree.statements.filter(ts.isImportDeclaration).flatMap(n=>n.importClause?.namedBindings&&ts.isNamedImports(n.importClause.namedBindings)?n.importClause.namedBindings.elements.filter(e=>e.name.text===name).map(e=>({body:n.getText(tree),module:n.moduleSpecifier.text,original:e.propertyName?.text||e.name.text,type:!!(n.importClause.isTypeOnly||e.isTypeOnly)})):[]);assert.equal(a.length,1);assert.equal(a[0].module,module);assert.equal(a[0].original,name);assert.equal(a[0].type,false);return a[0]});
const inputPaths=['packages/safe-cex/src/types.ts','packages/safe-cex/src/exchanges/lighter/lighter.types.ts','packages/safe-cex/src/utils/get-key.ts'],inputs=inputPaths.map(f=>fs.readFileSync(path.join(root,f),'utf8'));
function parse(s){const t=ts.createSourceFile('input.ts',s,ts.ScriptTarget.Latest,true);assert.equal(t.parseDiagnostics.length,0);return t}
function named(t,names){return names.map(name=>{const a=t.statements.filter(n=>n.name?.getText(t)===name||ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(t)===name));assert.equal(a.length,1,'unique '+name);return a[0].getText(t)}).join('\n')}
function compile(s){const out=ts.transpileModule(s,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},reportDiagnostics:true});assert.equal(out.diagnostics.length,0);return out.outputText}
const aux=vm.createContext({exports:{}}),enumNames=['OrderType','OrderSide','OrderStatus','OrderTimeInForce'];vm.runInContext(compile(named(parse(inputs[0]),enumNames)+'\n'+named(parse(inputs[1]),['ORDER_TYPE','ORDER_SIDE','ORDER_STATUS','ORDER_TIME_IN_FORCE'])),aux);
const lodashNames=['camelCase','memoize','snakeCase'],lodashModules=lodashNames.map(name=>require.resolve('lodash/'+name,{paths:[process.env.SAFE_CEX_LODASH_ROOT||path.join(root,'node_modules')]})),lodashVersion=require(path.join(path.dirname(lodashModules[0]),'package.json')).version;
const keyModule={exports:{}};vm.runInNewContext(compile(inputs[2]),{module:keyModule,exports:keyModule.exports,require(name){assert.ok(lodashNames.some(n=>name==='lodash/'+n));return require(lodashModules[lodashNames.findIndex(n=>name==='lodash/'+n)])}});const actualV=keyModule.exports.v;
const body=process.env.LIGHTER_ORDER_MAPPING_MUTATION==='remaining'?original.replace('remaining = amount;','remaining = filled;'):original;
if(process.env.LIGHTER_ORDER_MAPPING_MUTATION==='remaining')assert.notEqual(body,original,'source mutation must replace one actual body expression');
// Instrument allocations/iteration only; the maintained business statements stay intact.
const t=parse('class Subject {'+body+'}exports.Subject=Subject;');
const transformed=ts.transform(t,[ctx=>{const f=ctx.factory;function visit(node){
 if(ts.isObjectLiteralExpression(node)){const v=ts.visitEachChild(node,visit,ctx);return f.createCallExpression(f.createIdentifier('__literal'),undefined,[v,f.createStringLiteral(node.properties.some(ts.isSpreadAssignment)?'copy':'literal')])}
 if(ts.isArrayLiteralExpression(node))return f.createCallExpression(f.createIdentifier('__array'),undefined,[ts.visitEachChild(node,visit,ctx)]);
 if(ts.isRegularExpressionLiteral(node))return f.createCallExpression(f.createIdentifier('__regex'),undefined,[node]);
 if(ts.isArrowFunction(node)&&ts.isBlock(node.body)){const updated=ts.visitEachChild(node,visit,ctx),rest=node.parameters.filter(p=>p.dotDotDotToken);if(rest.length)return f.updateArrowFunction(updated,updated.modifiers,updated.typeParameters,updated.parameters,updated.type,updated.equalsGreaterThanToken,f.updateBlock(updated.body,[...rest.map(p=>f.createExpressionStatement(f.createCallExpression(f.createIdentifier('__array'),undefined,[f.createIdentifier(p.name.getText(t))]))),...updated.body.statements]));return updated}
 return ts.visitEachChild(node,visit,ctx)}return n=>ts.visitNode(n,visit)}]);
const scope=vm.createContext(vm.constants.DONT_CONTEXTIFY);scope.exports={};vm.runInContext(compile(ts.createPrinter().printFile(transformed.transformed[0])),scope);transformed.dispose();
const SourceArray=vm.runInContext('Array',scope),SourceString=vm.runInContext('String',scope),SourceNumber=vm.runInContext('Number',scope),SourceObject=vm.runInContext('Object',scope),SourceBigInt=vm.runInContext('BigInt',scope),SourceError=vm.runInContext('Error',scope),SourceRegExp=vm.runInContext('RegExp',scope),arrayIterator=SourceArray.prototype[Symbol.iterator],arrayFind=SourceArray.prototype.find,objectEntries=SourceObject.entries,bigString=vm.runInContext('BigInt.prototype.toString',scope),regexTest=SourceRegExp.prototype.test;
const stringMethods=Object.fromEntries(['replace','trim','toLowerCase','toUpperCase'].map(n=>[n,SourceString.prototype[n]])),units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));let c;const symbolValue=Symbol('mapping/\ud800');
function tag(v){if(v===undefined)return{kind:'undefined'};if(v===null)return{kind:'null'};if(typeof v==='boolean')return{kind:'boolean',value:v};if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return{kind:'number',bits:b.toString('hex')}}if(typeof v==='bigint')return{kind:'bigint',value:String(v)};if(typeof v==='string')return{kind:'string',units:units(v)};if(typeof v==='symbol'){assert.equal(v,symbolValue);return{kind:'symbol'}}if(typeof v==='function'){if(!c.ids.has(v))c.ids.set(v,'Callback'+c.callbacks++);return{kind:'function',id:c.ids.get(v)}}if(!c.ids.has(v))register(v,'Host'+c.host++);return{kind:'object',id:c.ids.get(v)}}
function register(value,id,kind){c.ids.set(value,id);const raw=c.raw.get(value)||value;const record={id,array:Array.isArray(raw),error:value instanceof SourceError,fields:[],kind:kind||'host'};c.objects.push(record);c.suppress++;try{record.fields=Object.keys(raw).map(key=>({keyUnits:units(key),value:tag(raw[key])}));}finally{c.suppress--}return record}
function completion(v,ok){if(ok)return{kind:'ok',value:tag(v)};if(v===c.thrown)return{kind:'thrown'};return{kind:'generated',name:v.name,message:units(v.message)}}
function event(e,work){if(c.suppress)return work();e.depth=c.depth;const selector=e.op+':'+(e.name||((e.object?.id||'')+'.'+e.key));if(e.key!==undefined){e.keyUnits=units(e.key);delete e.key}c.events.push(e);c.seen[selector]=(c.seen[selector]||0)+1;if(c.i.throwAt===selector&&c.seen[selector]===(c.i.throwNth||1)){c.injected++;e.error={kind:'thrown'};throw c.thrown}try{const value=work();e.result=tag(value);return value}catch(error){e.error=completion(error,false);throw error}}
function invoke(name,receiver,args,callee,work){try{const v=event({op:'call',name,callee:tag(callee),receiver:tag(receiver),args:args.map(tag)},()=>{c.depth++;try{return work()}finally{c.depth--}});c.events.push({op:'end-call',depth:c.depth});return v}catch(e){c.events.push({op:'end-call',depth:c.depth});throw e}}
function callable(name,receiver,work){const f=function(...args){assert.equal(this,receiver,'captured receiver '+name);return invoke(name,receiver,args,f,()=>work(...args))};c.ids.set(f,'C.'+name+'.'+c.callables++);return f}
function global(name,work){Object.defineProperty(scope,name,{configurable:true,get(){return event({op:'global',name},()=>callable(name,undefined,work))}})}
function namespace(name,methods){const obj={};for(const[key,work]of Object.entries(methods))Object.defineProperty(obj,key,{get(){return event({op:'global',name:name+'.'+key,receiver:tag(obj)},()=>callable(name+'.'+key,obj,work))}});scope[name]=obj;c.ids.set(obj,name);register(obj,name);return obj}
function object(id,fields,methods={}){const target={...fields};let proxy;proxy=new Proxy(target,{get(t,key,receiver){if(typeof key==='symbol')return Reflect.get(t,key,receiver);if(methods[key])return event({op:'member',object:{kind:'object',id},key,name:id+'.'+key},()=>callable(id+'.'+key,proxy,methods[key]));return event({op:'get',object:{kind:'object',id},key},()=>Reflect.get(t,key,receiver))}});c.raw.set(proxy,target);register(proxy,id);return proxy}
function fresh(value,op){const id='Fresh'+c.fresh++;let result=value;if(!Array.isArray(value)&&op!=='regex'){result=new Proxy(value,{get(t,key,receiver){if(typeof key==='symbol')return Reflect.get(t,key,receiver);return event({op:'get',object:{kind:'object',id},key},()=>Reflect.get(t,key,receiver))}});c.raw.set(result,value)}register(result,id,op);c.constructs.push({id,op});return result}
scope.__literal=(v,op)=>fresh(v,op);scope.__array=v=>fresh(v,'array');scope.__regex=v=>{const result=fresh(v,'regex'),m=c.objects.find(o=>o.id===c.ids.get(result));m.pattern=v.source;m.flags=v.flags;return result};
Object.defineProperty(SourceArray.prototype,Symbol.iterator,{configurable:true,value:function(){if(c.suppress)return arrayIterator.call(this);const value=this,id=c.iterators++,iterator=event({op:'iter-open',name:String(id),object:tag(value)},()=>arrayIterator.call(value));return{next(){return event({op:'iter-next',name:String(id)},()=>iterator.next())},return(){return event({op:'iter-close',name:String(id)},()=>{c.closed++;return{done:true}})}}}});
for(const[key,work]of Object.entries(stringMethods))Object.defineProperty(SourceString.prototype,key,{configurable:true,get(){const receiver=this;return event({op:'member',object:tag(receiver),key,name:'String.'+key},()=>callable('String.'+key,receiver,(...args)=>work.call(receiver,...args)))}});
Object.defineProperty(vm.runInContext('BigInt.prototype',scope),'toString',{configurable:true,get(){const receiver=this;return event({op:'member',object:tag(receiver),key:'toString',name:'BigInt.toString'},()=>callable('BigInt.toString',receiver,(...a)=>bigString.call(receiver,...a)))}});
Object.defineProperty(SourceRegExp.prototype,'test',{configurable:true,get(){const receiver=this;return event({op:'member',object:tag(receiver),key:'test',name:'RegExp.test'},()=>callable('RegExp.test',receiver,(...a)=>regexTest.call(c.raw.get(receiver)||receiver,...a)))}});
Object.defineProperty(SourceArray.prototype,'find',{configurable:true,get(){const receiver=this;return event({op:'member',object:tag(receiver),key:'find',name:'Array.find'},()=>callable('Array.find',receiver,callback=>arrayFind.call(receiver,(...args)=>{const id=tag(callback).id;c.events.push({op:'invoke',depth:c.depth,id,args:args.map(tag)});try{const result=callback(...args);c.events.push({op:'invoke-result',depth:c.depth,result:completion(result,true)});return result}catch(e){c.events.push({op:'invoke-result',depth:c.depth,result:completion(e,false)});throw e}})))}});
function prepare(i){c={i,ids:new Map(),raw:new WeakMap(),objects:[],events:[],constructs:[],seen:{},depth:0,suppress:0,callbacks:0,callables:0,stringVersion:0,host:0,fresh:0,iterators:0,closed:0,injected:0};c.thrown={};c.ids.set(c.thrown,'Thrown');register(c.thrown,'Thrown');
 const base={order:undefined,orderTypeValue:'0',timeInForceValue:'1',postOnly:false,marketId:7,symbol:'BTC/\ud800',orderId:'17',orderIndex:null,initialBaseAmount:10,filledBaseAmount:0,remainingBaseAmount:0,quoteAmount:100,remainingQuoteAmount:null,side:'BUY',isBuy:null,isAsk:null,price:5,triggerPrice:null,status:'OPEN',reduceOnly:false,...i.patch};
 for(const key of i.omit||[])delete base[key];
 if(i.bigId)base.orderId=BigInt(i.bigId);if(i.symbolId)base.orderId=symbolValue;if(i.nanBuy)base.isBuy=NaN;if(i.nanReduce)base.reduceOnly=NaN;if(i.undefinedNormalized){delete base.initialBaseAmount;base['INITIAL-BASE-AMOUNT']=undefined;base['INITIAL BASE AMOUNT']=12;}if(i.infinityQuote)base.remainingQuoteAmount=Infinity;
 let nested;if(i.nested){nested=object('Nested',i.nested);base.order=nested}
 c.input=i.nullInput?null:i.undefinedInput?undefined:object('Input',base);if(i.changingOrder){const other=object('Nested2',{price:13});let reads=0;Object.defineProperty(c.raw.get(c.input),'order',{enumerable:true,get(){return reads++===0?nested:reads===2&&i.changingOrder==='notobject'?7:other}})}
 c.market=object('Market',{symbol:'BTC/\ud800'});c.byId=object('ById',{'7':i.noIdMarket?undefined:c.market});c.bySymbol=object('BySymbol',{'BTC/\ud800':i.noSymbolMarket?undefined:c.market});
 c.hint=i.hint?object('Hint',i.hint):undefined;if(i.hintTypeSeries){let reads=0;Object.defineProperty(c.raw.get(c.hint),'type',{enumerable:true,get(){const series=i.hintTypeSeries;return series[Math.min(reads++,series.length-1)]}})}const existing=i.existing?object('Existing',i.existing):undefined;c.orders=SourceArray.of(...(existing?[existing]:[]));register(c.orders,'Orders');
 c.store=object('Store',{marketsById:c.byId,marketsBySymbol:c.bySymbol,orders:c.orders});const owner=new scope.exports.Subject();c.ids.set(owner,'Owner');register(owner,'Owner').fields=[{keyUnits:units('store'),value:tag(c.store)}];Object.defineProperty(owner,'store',{get(){return event({op:'get',object:tag(owner),key:'store'},()=>c.store)}});
 for(const[name,work]of [['readPlacedOrderHint',()=>c.hint],['inferStopLossTypeFromReduceOnlyLimit',()=>i.inferred]])Object.defineProperty(owner,name,{get(){return event({op:'member',object:tag(owner),key:name,name:'Owner.'+name},()=>callable('Owner.'+name,owner,work))}});c.owner=owner;
 for(const name of enumNames){const raw=aux.exports[name];scope[name]=new Proxy(raw,{get(t,key){return event({op:'constant',name:name+'.'+key},()=>t[key])}})}
 for(const name of ['ORDER_TYPE','ORDER_SIDE','ORDER_STATUS','ORDER_TIME_IN_FORCE']){const value=object(name,aux.exports[name]);Object.defineProperty(scope,name,{configurable:true,get(){return event({op:'constant',name},()=>value)}})}
 global('v',(obj,key)=>{const value=actualV(obj,key);if(i.replaceString&&key==='symbol')c.stringVersion=1;return value});Object.defineProperty(scope,'String',{configurable:true,get(){const version=c.stringVersion;return event({op:'global',name:'String'},()=>callable('String',undefined,v=>(version?'new/':'')+SourceString(v)))}});global('Boolean',v=>Boolean(v));global('BigInt',v=>SourceBigInt(v));
 global('Number',v=>SourceNumber(v));const getNumber=Object.getOwnPropertyDescriptor(scope,'Number').get;Object.defineProperty(scope,'Number',{configurable:true,get(){const f=getNumber();for(const key of ['parseFloat','isFinite'])Object.defineProperty(f,key,{get(){return event({op:'global',name:'Number.'+key,receiver:tag(f)},()=>callable('Number.'+key,f,v=>SourceNumber[key](v)))}});return f}});
 namespace('Object',{entries:v=>objectEntries(v)});namespace('Math',{max:(...v)=>Math.max(...v)});namespace('console',{warn:()=>undefined});namespace('virtualClock',{getCurrentTimeMs:()=>10000});
 c.events=[];c.constructs=[];
}
const cases=[{},
 ...['0','1','2','3','4','5','stop-market','take profit','TRAILING_STOP'].map(orderTypeValue=>({patch:{orderTypeValue}})),
 ...['0','1','2'].map(timeInForceValue=>({patch:{timeInForceValue}})),
 {patch:{postOnly:'false'}},{patch:{orderTypeValue:'',timeInForceValue:'',side:'',price:0},hint:{type:'stop_market',timeInForce:'ImmediateOrCancel',side:'buy',price:9,reduceOnly:true}},
 {existing:{id:'17',type:'take_profit_market',timeInForce:'ImmediateOrCancel',side:'sell',reduceOnly:true},patch:{orderTypeValue:'',timeInForceValue:'',side:'',price:0,triggerPrice:8}},
 {patch:{orderId:'00017'}},{bigId:'9223372036854775807'},{bigId:'9223372036854775808',patch:{orderIndex:'18'}},{patch:{orderId:'-1',orderIndex:'19'}},{patch:{orderId:'1e3',orderIndex:null}},{symbolId:true},
 {patch:{initialBaseAmount:null,filledBaseAmount:2,remainingBaseAmount:3}},{patch:{filledBaseAmount:null,remainingBaseAmount:4}},{patch:{filledBaseAmount:4,remainingBaseAmount:null}},{patch:{status:'FILLED'}},
 {patch:{initialBaseAmount:0,remainingBaseAmount:0,remainingQuoteAmount:25}},{infinityQuote:true},{patch:{initialBaseAmount:'2.5tail',filledBaseAmount:'NaN',remainingBaseAmount:null}},
 ...[true,false,0,1,-1,'true','TRUE','false','0',' 1 ','other',null].map(reduceOnly=>({patch:{reduceOnly},hint:{reduceOnly:true}})),
 {patch:{side:'',isAsk:false,isBuy:false}},{patch:{side:'',isAsk:true,isBuy:true}},{patch:{side:'',isBuy:0}},{nanBuy:true,patch:{side:''}},{patch:{side:''},hint:{side:'buy'}},
 {noIdMarket:true},{noIdMarket:true,noSymbolMarket:true},{nested:{price:11,remainingBaseAmount:2}},
 {omit:['initialBaseAmount'],patch:{' INITIAL - BASE _ AMOUNT ':12}},
 {patch:{orderId:null,orderIndex:null}},{inferred:'stop_market'},{nullInput:true},{undefinedInput:true}
];
const faults=[['global:String'],['global:Number'],['global:Number.parseFloat'],['call:Number.parseFloat'],['member:String.trim'],['global:v'],['call:v'],['constant:OrderType.StopLoss'],['constant:ORDER_TYPE'],['get:Input.order'],['get:Store.marketsById'],['get:ById.7'],['member:Owner.readPlacedOrderHint'],['call:Owner.readPlacedOrderHint'],['member:BigInt.toString'],['global:BigInt'],['call:BigInt'],['member:RegExp.test'],['call:RegExp.test'],['global:virtualClock.getCurrentTimeMs'],['call:virtualClock.getCurrentTimeMs'],['get:Market.symbol'],['call:Owner.inferStopLossTypeFromReduceOnlyLimit'],['iter-open:0'],['iter-next:0'],['iter-close:0']];
for(const[throwAt]of faults)cases.push({throwAt});
cases.push(
 {undefinedNormalized:true},
 {changingOrder:'notobject',nested:{price:11}},
 {changingOrder:'replacement',nested:{price:11}},
 {hint:{type:'stop_market',timeInForce:'PostOnly',price:9},hintTypeSeries:['stop_market','limit','take_profit_market','stop_market','stop_market']},
 {throwAt:'call:BigInt.toString'},
 {throwAt:'iter-close:8'},
 {throwAt:'iter-next:8'},
 {nested:{price:11},throwAt:'get:Input.price'},
 {patch:{orderId:null,orderIndex:null},throwAt:'global:virtualClock.getCurrentTimeMs'},
 {patch:{orderId:'١٧',orderIndex:null}},
 {patch:{initialBaseAmount:-0}},
 {nanReduce:true},
 {patch:{reduceOnly:' FALSE '}},
 {hint:{type:'stop_market',timeInForce:'PostOnly',price:9},patch:{triggerPrice:10}},
 {hint:{type:'stop_market'},patch:{orderTypeValue:'1'}},
 {patch:{orderTypeValue:'stop\u00a0market'}},
 {patch:{status:'PENDING',remainingBaseAmount:0}},
 {patch:{status:'PARTIALLY_FILLED',remainingBaseAmount:-3,filledBaseAmount:4}},
 {hint:{price:9},patch:{price:null},inferred:'stop_market'}
);
cases.push({replaceString:true});assert.equal(cases.length,100,'independent authored spec100');
const rows=[];
for(const input of cases){prepare(input);const args=[tag(c.input)];let output;try{output=completion(c.owner.mapOrder(c.input),true)}catch(e){output=completion(e,false)}
 if(input.throwAt)assert.equal(c.injected,1,'authored throw reached '+input.throwAt);
 if(rows.length===0){const result=output.kind==='ok'?c.objects.find(o=>o.id===output.value.id):undefined;assert.ok(result,'authored first mapped array: '+JSON.stringify(output));assert.equal(result.fields.length,1,'authored mapped count1');const order=c.objects.find(o=>o.id===result.fields[0].value.id);const remaining=order.fields.find(f=>JSON.stringify(f.keyUnits)===JSON.stringify(units('remaining'))).value;assert.equal(remaining.bits,'4024000000000000','authored open remaining recovers amount10');assert.equal(order.fields.length,14,'authored mapped14fields');}

 const index=rows.length;
 if([17,18,19,80,81,82,83,84,91,93,94,95,96,97,98].includes(index)){
  assert.equal(output.kind,'ok','authored selected success '+index);const array=c.objects.find(o=>o.id===output.value.id);assert.equal(array.fields.length,1,'authored selected mapped1 '+index);const order=c.objects.find(o=>o.id===array.fields[0].value.id);const field=key=>order.fields.find(f=>JSON.stringify(f.keyUnits)===JSON.stringify(units(key))).value;
  const n=(key,value)=>{const b=Buffer.alloc(8);b.writeDoubleBE(value);assert.equal(field(key).bits,b.toString('hex'),'authored '+index+' '+key)};
  const str=(key,value)=>assert.deepEqual(field(key).units,units(value),'authored '+index+' '+key);
  if(index===17)str('id','9223372036854775807');if(index===18)str('id','18');if(index===19)str('id','19');if(index===80)n('amount',0);
  if(index===81){n('price',5);assert.equal(field('info').id,'Input','authored source original alias81')}
  if(index===82){n('price',13);assert.notEqual(field('info').id,'Input','authored source fresh copy82')}
  if(index===83){str('type','stop_market');str('timeInForce','PostOnly')}
  if(index===84)str('id','17');if(index===91)assert.equal(field('reduceOnly').value,true,'authored NaN boolean true91');
  if(index===93){str('type','stop_market');str('timeInForce','PostOnly');n('price',10)}
  if(index===94)str('type','market');if(index===95)str('type','stop_market');if(index===96||index===97)n('remaining',10);
  if(index===98){str('type','stop_market');n('price',9)}
 }
 if(index===99){const events=c.events;const symbol=events.findIndex(e=>e.op==='call'&&e.name==='v'&&JSON.stringify(e.args[1].units)===JSON.stringify(units('symbol')));assert.ok(symbol>0,'authored replacement symbol read99');const captured=events.slice(0,symbol).findLast(e=>e.op==='global'&&e.name==='String'),called=events.slice(symbol).find(e=>e.op==='call'&&e.name==='String');assert.equal(captured.result.id,called.callee.id,'authored original captured String callable99');assert.deepEqual(called.result.units,[66,84,67,47,55296],'authored captured String stays original after replacement99');assert.equal(c.stringVersion,1,'authored live String replaced99')}
 if([85,86,87,88].includes(index))assert.equal(output.kind,'thrown','authored original throw '+index);
 if([20,21,50,89,99].includes(index)){assert.equal(output.kind,'ok','authored empty success '+index);assert.equal(c.objects.find(o=>o.id===output.value.id).fields.length,0,'authored empty result '+index)}
 rows.push({method:'mapOrder',input:JSON.parse(JSON.stringify(input)),args,objects:c.objects,steps:[{events:c.events,constructs:c.constructs,output}],closed:c.closed});}
const pin=crypto.createHash('sha256').update(JSON.stringify({original,imports,inputs,lodashVersion})).digest('hex');process.stdout.write(JSON.stringify({rows,pin,lodashVersion}));
