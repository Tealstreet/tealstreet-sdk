// Execute the actual addInlineTpSlFallbackOrders initializer and imported source helpers.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH,'compiler_required');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root=path.resolve(__dirname,'../../..');
const external=require('node:module').createRequire(require.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH));
const context=vm.createContext({structuredClone,process,setTimeout,clearTimeout,console});
const cache=new Map();
function execute(file) {
  if(cache.has(file))return cache.get(file);
  const exports={};cache.set(file,exports);
  const output=ts.transpileModule(fs.readFileSync(file,'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,esModuleInterop:true}}).outputText;
  vm.runInContext(`(function(exports,require){${output}\n})`,context,{filename:file})(exports,spec=>{
    if(spec.startsWith('@tealstreet/'))return execute(path.join(root,'packages',spec.slice('@tealstreet/'.length)+'.ts'));
    if(!spec.startsWith('.'))return external(spec);
    const target=path.resolve(path.dirname(file),spec);
    return execute(fs.existsSync(target+'.ts')?target+'.ts':path.join(target,'index.ts'));
  });return exports;
}
const file=path.join(root,'packages/safe-cex/src/exchanges/blofin/blofin.exchange.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const klass=ast.statements.find(s=>ts.isClassDeclaration(s)&&s.name.text==='BlofinExchange');
const member=klass.members.find(m=>m.name?.text==='addInlineTpSlFallbackOrders');
assert(member&&ts.isPropertyDeclaration(member)&&ts.isArrowFunction(member.initializer),'inline_source_shape');
Object.assign(context,execute(path.join(root,'packages/safe-cex/src/types.ts')));
const clock=[];context.virtualClock={getCurrentTimeMs(){const value=1700000000000.125+clock.length*0.375;clock.push(value);return value;}};
const factory=vm.runInContext(ts.transpileModule(`(function(){return ${member.initializer.getText(ast)};})`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context,{filename:file});
function observe(value){
 if(typeof value==='string'){
  for(let i=0;i<value.length;i++){const c=value.charCodeAt(i);if(c>=0xd800&&c<=0xdbff){const next=value.charCodeAt(i+1);if(next>=0xdc00&&next<=0xdfff){i++;continue;}return{utf16:Array.from({length:value.length},(_,n)=>value.charCodeAt(n))};}if(c>=0xdc00&&c<=0xdfff)return{utf16:Array.from({length:value.length},(_,n)=>value.charCodeAt(n))};}
 }

 if(value===undefined)return{undefined:true};
 if(typeof value==='function')return{function:value.name};
 if(value===Object.prototype)return{prototype:true};
 if(typeof value==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(value);return{number:Number.isNaN(value)?'nan':b.toString('hex')};}
 if(Array.isArray(value))return Array.from(value,observe);
 if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,observe(v)]));return value;
}
const base={id:'parent-one',status:'open',symbol:'BTCUSDT',type:'limit',side:'buy',price:11.375,amount:17.25,filled:2.125,remaining:15.125,reduceOnly:false,lastUpdate:19.625,extra:{remainingNative:23.875,filledInPlacementUnits:27.125},info:{orderCategory:'normal',tpTriggerPrice:'29.625tail',slTriggerPrice:'7.375tail'}};
const tp={...base,id:'parent-one__take_profit',type:'take_profit_market',info:{orderId:'parent-one'}};
const sl={...base,id:'source-stop',type:'stop_market',info:{orderId:'parent-one'}};
const cases=[];
function run(raw){const input=JSON.parse(raw);clock.length=0;const fn=factory();let result,prefixAliases=[],infoAliases=[];try{const output=fn(input);prefixAliases=input.map((o,i)=>o===output[i]);for(let i=input.length;i<output.length;i++)infoAliases.push(input.findIndex(o=>o.info===output[i].info));result={ok:observe(output)};}catch(error){result={error:error.name,message:error.message};}cases.push({raw,result,clock:observe(clock),prefixAliases,infoAliases});}
run('[]');run(JSON.stringify([base]));
for(const side of ['buy','sell'])for(const type of ['limit','market','conditional','stop_market','take_profit_market','trailing_stop_market'])run(JSON.stringify([{...base,side,type}]));
for(const amount of [0,-1,0.125,17.25])for(const remaining of [0,-1,0.375,19.875])for(const native of [undefined,0,-1,0.625])run(JSON.stringify([{...base,amount,remaining,extra:native===undefined?undefined:{remainingNative:native}}]));
for(const type of ['tpTriggerPrice','slTriggerPrice','orderCategory','tpslId']){
 const values=['null','true','false','0','-0','1e400','-1e-400','9007199254740993','[]','["2.75tail"]','{}','{"toString":null}',...['',' ','-0','2.5tail','Infinity','NaN','normal','雪😀'].map(JSON.stringify)];
 for(const value of values){const props=Object.entries({...base.info,[type]:null}).map(([k,v])=>JSON.stringify(k)+':'+(k===type?value:JSON.stringify(v)));run(JSON.stringify([{...base,info:'RAW_INFO'}]).replace('"RAW_INFO"','{'+props.join(',')+'}'));}
 const info={...base.info};delete info[type];run(JSON.stringify([{...base,info}]));
}
for(const orderId of ['',null,0,false,[],{},'parent-one',' parent-one ','\ufeffparent-one\u2028','\u0085parent-one\u0085',{toString:null}])for(const id of ['source-tp','parent-one__take_profit','parent-one__x__z','__empty','other__take_profit'])run(JSON.stringify([base,{...tp,id,info:{orderId}}]));
for(const orders of [[base,tp],[tp,base],[base,sl],[sl,base],[base,tp,sl],[tp,sl,base],[base,base],[base,{...base,id:'parent-two'}],[{...base,id:''}],[{...base,id:'a:b__c'},tp],[{...base,info:null}],[{...base,info:false}],[{...base,info:[]}],[{...base,info:'text'}]])run(JSON.stringify(orders));
for(const field of ['amount','remaining'])for(const value of ['-0','1e400','-1e400','-1e-400','9007199254740993'])run(JSON.stringify([{...base,[field]:'RAW_NUMBER'}]).replace('"RAW_NUMBER"',value));
run(JSON.stringify([{...base,info:{...base.info,opaque:'RAW_NUMBER'}}]).replace('"RAW_NUMBER"','-0'));
for(const status of Object.values(context.OrderStatus))for(const timeInForce of Object.values(context.OrderTimeInForce))run(JSON.stringify([{...base,status,timeInForce,parentId:'native-parent',close:true,triggerType:'mark_price',tpTriggerType:'index_price',slTriggerType:'last_price',takeProfit:31.125,stopLoss:3.875,extra:{...base.extra,remainingNotional:33.625,conditionalOrderPrice:35.375,rejectReason:'PostOnly',isFromChaser:true,chaserId:'chaser-unique',chaserAttempts:37.25,chaserMaxAttempts:39.75}}]));
// Typed ingress refuses these runtime-admitted malformed Order values; observe actual source.
const boundaries=[];
for(const raw of [JSON.stringify([{...base,remaining:'15.125'}]),JSON.stringify([{...base,side:'other'}]),JSON.stringify([{...base,extra:{remainingNative:'23.875'}}]),JSON.stringify([{...base,price:null}]),JSON.stringify([{...base,id:'SURROGATE'}]).replace('SURROGATE','\\ud800'),'['.repeat(140)+'0'+']'.repeat(140)]){
 let result;try{const output=factory()(JSON.parse(raw));result=raw.startsWith('[[')?{ok:true,sourceOutputLength:output.length}:{ok:observe(output)}}catch(error){result={error:error.name,message:error.message};}boundaries.push({raw,result});
}
const projectionRaw=JSON.stringify([{...base,unknownOrder:'source-keeps-order',extra:{...base.extra,unknownExtra:'source-keeps-extra'}}]);
const projectionSource=observe(factory()(JSON.parse(projectionRaw)));
process.stdout.write(JSON.stringify({projection:{raw:projectionRaw,source:projectionSource},header:{architecture:process.arch,nodeMajor:Number(process.versions.node.split('.')[0]),nodeVersion:process.version},cases,boundaries}));
